# AURA-ML-5100 - A region was unusable, so a small fix was skipped

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Some photographs have fewer small fixes than others, and those frames carry
`micro_region_unavailable` or `micro_region_doubtful` in the panel.

## What actually happened

Phase 21 has exactly one route to a region - the `MicroField` port that phase 18 fills - and
**no geometric fallback**. That is deliberate, and it is phase 19's argument inherited twice: a
rectangle's edge does not follow a person, and a teeth correction through one whitens a lip.

Three states produce this code:

* **No region arrived at all.** On a build with no mask generator wired in, this is every frame,
  and `MicroOutline::region_covered` is zero. That is the honest reading of such a build.
* **The region arrived and is unreadable** - a side of zero, an alpha length that does not match
  the dimensions, a confidence outside its range. This is a bug in the producer.
* **The region arrived and is too doubtful.** `MicroField::strength_scale` is zero below
  `MIN_MASK_CONFIDENCE`, and the operation is skipped rather than run gently.

The difference between the first and "there was nothing to fix" is the whole point of this code.
Those two look identical in a coverage report otherwise, and they send a support engineer to two
different places.

## What to do

1. Check `MicroOutline::region_covered` against `MicroOutline::planned`. A low ratio is a phase 18
   problem, not a phase 21 problem.
2. If the field is unreadable rather than absent, the producer is at fault: see
   `docs/runbooks/AURA-ML-5086.md` and phase 18's own gates.
3. Nothing to do per frame. A frame nobody could locate a region in ships as it was.
