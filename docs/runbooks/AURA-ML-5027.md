# AURA-ML-5027 - One photograph could not be classified

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, aggregated to a count in the Problems list rather than repeated per frame. The wedding is unaffected.

## What actually happened

The scene pass reached one photograph and could not produce a posterior for it. The classifier sits on the phase 05 embedding trunk, so the ordinary causes are all upstream of the head:

1. **The photograph has no embedding.** The scene head consumes a vector, not pixels - that is the whole reason section 11 budgets 35 s for four thousand images. A frame that phase 05 skipped has nothing to classify. Look for `AURA-ML-5014` or an incomplete embedding pass first; `IndexStatusDto.coverage` is the fast check.
2. **The embedding is from a different `MODEL_VER` or `PREPROCESS_VER`.** Refused rather than used - see `AURA-ML-5022`.
3. **The context features could not be read.** Hour-of-day needs `photo.timeline_time`, the ISO bucket and flash flag need EXIF, and the face count needs a `face_scan` row. Each of those is *optional* and substitutes a documented neutral value; this cause only fires when the catalog read itself failed, which means `AURA-DB-3006` is beside it.
4. **The inference call failed** - `AURA-ML-5008` on a deadline, `AURA-ML-5011` on cancellation, `AURA-GPU-4xxx` on a device fault.

## What AURA does automatically

Writes nothing for that frame - deliberately, so the next pass retries it rather than treating an `unknown` row as done - counts it in `ScenePassReport::failed`, and continues. The frame simply has no scene, `StoryService::scene` returns `None` for it, and every later phase falls back to its non-scene path and says so in its own reasons.

A frame with no scene still lands in a chapter: `segment_images` membership is by timeline position, not by label.

## Operator steps

1. Compare `v_scene_coverage.classified` with `v_embedding_coverage`. If the second is lower, this is not a scene problem.
2. A whole card failing at once is cause 1 or 4, never cause 3. Read the code logged immediately before the first failure.
3. Re-run the pass. It is resumable and idempotent: only frames without a current-version row are touched.
4. A single frame that fails on every pass is worth looking at directly - it is usually a photograph that also failed to embed, and `AURA-RAW-2xxx` will be in its history.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The embedding pass this one depends on: `docs/runbooks/AURA-ML-5014.md`
- Version drift: `docs/runbooks/AURA-ML-5022.md`
