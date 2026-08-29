# AURA-ML-5112 - A region was unusable, so sharpening was skipped

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs that were not sharpened, carrying `restore_sharpen_no_regions` or
`restore_region_unusable` in the panel.

## What actually happened

Deconvolution sharpening in this phase requires regions from phase 18 and has **no fallback**
that sharpens the whole frame at a lower amount. That is a deliberate decision, and ADR-0047
section 4 records the argument: skin, sky and out-of-focus background are not regions where
sharpening is less welcome, they are the three regions where it is *visible as damage*, so an
unmasked global sharpen spends its whole artefact budget on the three places a photographer
looks first.

Two causes:

* **no field arrived at all**, which on this build is the ordinary case - phase 18 segmenter is
  an untrained placeholder and `AppState` wires no generator into this pass;
* **a field arrived and could not be read** - a zero side, a length that does not match the
  dimensions, or a confidence outside `0..1`. `RestoreField::problem` names which.

Denoising and face recovery are unaffected. Neither needs a region.

## What to do

1. On this build, nothing. `RestoreOutline::region_covered` is zero, and condition C3 of
   `docs/progress/PHASE-22-EXIT.md` is what closes it.
2. When a mask generator is wired in, a project with low `region_covered` is a phase 18 problem:
   `aura-cli verify --phase 18` reports its own coverage.
