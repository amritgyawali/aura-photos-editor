# AURA-ML-5050 - Photographs had no analysis and were not considered

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A count in the coverage panel: how many photographs were left out because nobody had
checked them. They are marked `not_analysed`, which the panel renders differently from a
rejection.

## Why this is a warning and not a rejection

"Not checked" is not "not good enough". A gallery missing four hundred frames because a
preview pass was interrupted looks identical to a gallery missing four hundred frames
because they were soft - and only one of those is a decision. `CullOutline::coverage` is
the number that tells them apart, and its denominator is every photograph in the project.

## Operator steps

1. Read `CullOutline::coverage`. Below roughly 0.95 the gallery is a gallery of part of a
   wedding.
2. Run the phase 09 integrity pass to completion; it is the pass that makes a frame
   eligible.
3. Check phases 10 and 11 too - `emotion_aware` and `composition_aware` say how much of
   the fusion was real, and a cull at 3 % emotion-aware was made on two of four signals.
4. Re-run the cull. Frames that gained a verdict join the gallery; hand overrides survive.

Never deliver a gallery whose coverage is unknown. The gap is invisible in the grid.
