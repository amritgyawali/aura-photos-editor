# AURA-ML-5121 - A removal was undone by the artefact self-check before it was shown

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing. That is the point of this code: the proposal never reaches the review queue, so nobody is
asked to judge a repair the product already knew was bad. The count appears in
`CleanupOutline::reverted` and in the Cleanup panel's summary.

## What actually happened

`selfcheck::inspect` ran over the **rendered** result and found one of three things:

* texture repeated at a spatial period that occurs nowhere else in the frame;
* a straight line whose direction changes inside the patched region;
* a gradient that terminates at the patch boundary.

Each is compared against the *rest of the frame* rather than against the region's own before-state.
That is not a detail. An inpaint necessarily changes the pixels inside its own region, so anything
that measures how much they changed measures the removal rather than the artefact - the same trap
phase 19 hit with its halo test and phase 22 hit with its ringing test. ADR-0049 section 8.

## Why this is a warning rather than a failure

It is the mechanism working. A photographer who sees this number go up is seeing the product decline
to ship an artefact, which is the guarantee section 10.1 asks for expressed as a count.

## When to worry

If the reverted count approaches the proposed count, the fill or the borrow is producing bad
patches rather than the self-check being strict. Compare `reverted` against `applied` in the
outline; a healthy project has far more applied than reverted.
