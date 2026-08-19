# AURA-ML-5071 - A mask was unusable, so a local operation was scaled down or skipped

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Frames carrying the `mask_unavailable` or `mask_weak` reason, a lower plan confidence, and -
this is the point - a *gentler* edit rather than an artefact. `LocalOutline::mask_covered`
is the project-level number.

## What actually happened

**Phase 19 does not own a mask.** Phase 18 generates them; `MaskField` is the input port this
phase reads them through, and there is no mask generator, no segmentation model and no
geometric fallback anywhere in `aura-brain-photo::local`. A second answer to "where does the
subject end" is a background reduction that traces an outline nothing else in the product
agrees with, which is exactly the halo this phase exists to avoid.

So when a field does not arrive, or arrives unreadable, or arrives with a confidence below
`MIN_MASK_CONFIDENCE`, the operations that needed it are **gated rather than guessed**:

* below `MIN_MASK_CONFIDENCE` the operation is skipped and named in
  `LocalLightPlan::gated_by_mask_quality`;
* between there and `FULL_MASK_CONFIDENCE` the strength ramps linearly and is multiplied by
  the edge quality;
* an unreadable field - dimensions that disagree with the alpha length, a side above
  `MaskField::MAX_SIDE`, a confidence outside `0..1` - is treated as absent.

**On this build every frame gets here**, because phase 18 has not shipped. That is condition
C1 in the phase 19 exit report and it is not a fault to investigate.

## Operator steps

1. `LocalOutline::gated_histogram` says which mask kind is missing, in `MaskKind::ALL` order.
2. If the count is the whole project and phase 18 has not shipped, this is expected; see the
   exit report.
3. If phase 18 *has* shipped, check its own coverage first - a low mask coverage is a phase 18
   result reported here, and looking for the cause in phase 19 is the wrong place.
