# AURA-ML-5081 - A mask's quality limits what may be done with it

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The mask panel shows the region with an amber quality bar and one sentence naming what is
limiting it - the class or the boundary. Local tone changes through that region are applied at
reduced strength; skin smoothing and generative cleanup are unavailable and say so.

## What actually happened

`mask::quality::allowance` computed a ceiling below one. The ceiling is the **geometric mean** of
`confidence` and `edge_quality`, so either of them being low takes it down and neither can
rescue the other - phase 12 fused its four sub-scores the same way and for the same reason.

Below `AGGRESSIVE_FLOOR` the two operations section 6.4 names - skin smoothing and generative
cleanup - are refused outright. Everything else is *scaled*, not refused, because a threshold
below which nothing applies turns a graded response into a cliff, and a cliff is what silently
leaves half a gallery unedited.

**This is the first code in the product that constrains a later phase.** Everything before it
reported a decision AURA had already made. This one is read by phases 19 to 24 before they
decide anything, and it is how a boundary the photograph does not really contain stops a retouch
from being applied through it.

## Operator steps

1. Look at which of the two numbers is low. They are fixed by different things:
   * low **confidence** means AURA is unsure what the region *is* - a dark suit against a dark
     background, a face at the edge of a crowd. Brushing the region by hand fixes it, and a
     hand-edited mask is never gated.
   * low **edge quality** means the boundary is not well determined - backlight, motion, a veil.
     Refine Edge in the panel re-runs the matte with a wider band; brushing also works.
2. A hand-edited mask returns an allowance of 1.0. A photographer who has drawn the region is the
   authority on it.

## What would make this impossible

Nothing, and it should not. A mask that is not good enough for an operation is the ordinary case
on a hard photograph, and the point of this code is that the product says so instead of applying
the operation and producing an artefact.
