# AURA-ML-5118 - One photograph could not be examined for distractions

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

That one photograph carries no proposals and is delivered exactly as it was shot. Every other
photograph in the wedding is unaffected.

## What actually happened

The cleanup pass could not complete for one image: its proxy would not decode, its masks could not
be read, or the store rejected the write.

## Why it is `retry` and not `fallback`

There is no fallback for this phase, because the fallback *is* the default. A photograph nobody
examined and a photograph with nothing to remove are delivered identically - which is why
`CleanupOutline::examined` and `CleanupOutline::coverage` exist, and why a panel must never render
"no distractions found" for a frame that raised this.

## What to check

Run `aura-cli verify --phase 24` against the project. If a specific image repeats, its proxy is the
first thing to look at; `docs/runbooks/previews.md` covers that path.
