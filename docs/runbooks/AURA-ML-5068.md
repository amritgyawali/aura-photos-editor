# AURA-ML-5068 - One photograph could not be graded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One frame in the grid has no contrast or colour adjustment and the Tone panel says AURA has
not graded it yet. Every other frame in the wedding is unaffected.

## What actually happened

The grading pass could not read the frame's 2048 px proxy, or the proxy it read was not
8-bit sRGB. Every statistic in this phase is defined over an encoded sRGB proxy; reading a
16-bit linear buffer as one produces a plausible and wrong answer rather than a failure, so
the analyser refuses instead.

**No row is written.** That is deliberate and it is the same choice phase 15 made: a
written-but-neutral decision reads to phases 17, 25 and 27 as "AURA decided this photograph
needed nothing", and all three act on that. A missing row is pending work; a neutral row is
a wrong answer.

## Operator steps

1. Check `docs/runbooks/previews.md` first. This code almost always means the preview
   pipeline failed for that file, and the cause is there rather than here.
2. Re-run the pass. `ColourStore::pending` is a query, so the frame is picked up again with
   no bookkeeping.
3. If the same frame fails repeatedly, check whether its RAW decodes at all -
   `docs/camera-support.md` lists what this build decodes and what falls back.
