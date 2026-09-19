# AURA-ML-5148 - A look was measured against a different render engine or measurer

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The look is listed, marked as measured by an earlier version of AURA, and **not applied**. Their
photographs render exactly as phases 15 and 16 decided, which is what they would get with no look
selected at all.

## What actually happened

A look is a *difference between two appearance measurements* - what a page does, and what AURA
would have done to this photographer's own photographs. Both halves were measured by a particular
build:

* `engine_ver` is the renderer the match was measured through. A look measured against one renderer
  and applied by another is a look whose measured dE00 is a figure about a build that no longer
  exists.
* `analysis_ver` is the measurer that produced every stored reading and aggregate. It is bumped on
  any change to `measure`, `light`, `aggregate` or `solve`, because all four decide what a stored
  number means.

Comparing across either returns a plausible number that means nothing. This is the eighth
version-drift code in the product and it exists for the reason the other seven do: so that the
comparison never happens silently.

## What the product does about it

`Look::advise` returns the neutral delta and logs a warning. **Returning nothing rather than an
error is deliberate**: a stale look should make the product do nothing, not stop it. The wedding
renders, the panel explains, and the photographer decides when to act.

## What to do

Measure the look again, from the same folder. It takes as long as it did the first time - the
reference photographs are decoded again and a sample of the wedding is rendered twice - and it
produces a look this build can apply.

If the folder is gone, the look cannot be re-measured and should be deleted. There is deliberately
no way to "upgrade" a stored look in place: the numbers in it are measurements, and a migration
that rewrote them would be inventing measurements nobody took.
