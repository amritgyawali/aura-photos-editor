# AURA-ML-5083 - Stored masks came from a different model set or a different analysis

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The mask panel shows a background re-analysis running. Masks stay usable throughout. Any mask
edited by hand is untouched and keeps its own version stamp.

## What actually happened

`masks.model_ver` or `masks.analysis_ver` on the stored rows does not match this build's
`aura_vision::mask::MODEL_VER` or `ANALYSIS_VER`. They invalidate different things and that is
why there are two of them:

* **`model_ver`** invalidates *what a region is* - the class assignment. A new segmentation model
  is a new answer to "is that her hair".
* **`analysis_ver`** invalidates *where its boundary is* and how good it is - the trimap band,
  the matte, the two quality numbers. A change to the band radius is not a change to whether
  that is somebody's hair, and re-deciding every class because a radius moved is four thousand
  photographs of work nobody asked for.

This is the tenth version-drift code in the product, after `AURA-ML-5015`, `5018`, `5022`,
`5028`, `5033`, `5038`, `5060`, `5071` and `5077`, and it exists for the same reason all of them
do: a comparison across versions returns a plausible answer that means nothing, and it must never
happen silently.

## Operator steps

1. Let the background pass finish. The pending set is a query - `masks` left-joined at the
   current versions - so it is resumable and costs nothing on restart.
2. Nothing needs to be re-brushed. `user_edited = 1` is inside the `DELETE`'s own `WHERE`, so a
   regeneration cannot touch a photographer's own mask.

## What would make this impossible

One version column instead of two. It would make every band tweak a full re-segmentation, which
is the cost this split exists to avoid.
