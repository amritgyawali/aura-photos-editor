# AURA-ML-5078 - One photograph could not be masked

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The mask panel shows no regions for that one photograph, and the masking progress bar completes
with a count of what was skipped. Every other selected frame is masked normally. Nothing else in
the product changes: the frame is still in the gallery, still has its develop recipe, and still
renders.

## What actually happened

`Masks::mask_one` could not obtain the proxy for that photograph. The masking pass reads the
2048 px proxy through `FrameSource`, and the two ways that fails are a cache miss on a file that
has since moved and a decode that returns an error - both of which are `AURA-RAW-*` or
`AURA-IO-*` underneath, and this code is the wrapper that says which stage was asking.

**Nothing is written.** That is deliberate and it differs from phase 17's `AURA-ML-5072`, which
writes a rejected pair. A stored empty mask would read to phases 19 to 24 as "there is no skin
in this photograph" rather than as "nobody looked", and the distinction is exactly what
`MaskOutline::coverage` exists to report.

## Operator steps

1. Check whether the photograph's file is still where the catalog thinks it is:
   `aura-cli verify --phase 02` regenerates the proxy for one frame.
2. If the proxy is missing, rebuilding the preview cache fixes it - the mask pass will pick the
   frame up on its next run because the pending set is a query, not a journal.
3. If the decode itself fails, the underlying `AURA-RAW-*` code in the log is the real fault and
   `docs/camera-support.md` says whether that body decodes in this build.

## What would make this impossible

Nothing, and that is correct. A masking pass cannot fix a file it cannot read, and failing one
frame while finishing six hundred is the behaviour section 6.3's lazy policy is built around.
