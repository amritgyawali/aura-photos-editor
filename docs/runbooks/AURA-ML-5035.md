# AURA-ML-5035 - One photograph could not be checked

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence in the Problems panel, naming a count rather than a file. The photograph is in the grid, in its moment, with its faces and its scene - it simply carries no sharpness, motion, exposure or eye marks, and the Integrity card for it says "not checked" rather than "clean".

**That distinction is the whole point of this code.** A frame that could not be analysed must never be reported as a frame with nothing wrong, because phase 12 reads "nothing wrong" as evidence.

## What actually happened

One frame's analysis failed. In order of how often it happens:

1. **The proxy would not decode.** Phase 09 reads the 2048 px proxy, the same rung the face pass uses. A truncated or corrupt cache entry raises `AURA-RAW-2003` or `AURA-IO-1004` underneath and this code wraps it.
2. **The buffer was not 8-bit sRGB.** Every analyser in `aura-brain-photo` works on the documented proxy format; anything else is refused rather than reinterpreted.
3. **A learned head refused.** The focus head or the eye-state head raised `AURA-ML-5007` (shape) or was cancelled with `AURA-ML-5011`. A cancelled pass is not a failure and is reported separately.
4. **A face crop was unusable.** One bad crop does *not* fail the frame: the face is recorded with no eye state and the frame is scored without it. This appears in the log, not in this code.

## What AURA does automatically

Counts it, logs it with the photograph id, and continues. **No `image_integrity` row is written**, so the next pass tries the frame again - which is right for a transient decode failure and harmless for a permanent one. The same choice `AURA-ML-5017` makes for the face pass, and for the same reason.

The pass never stops. One unreadable frame must not end a 4,000-image run.

## Operator steps

1. Read the count. One frame in four thousand is a cache blip; four hundred is a broken cache directory or a wrong pixel format.
2. Re-run the pass. The pending set is a query, so a second run only touches what has no row.
3. If the same frame fails twice, take it out of the pipeline and check the proxy: `aura-cli previews --catalog ... --project ... --level proxy` rebuilds it.
4. `docs/runbooks/previews.md` covers a cache that is failing broadly.

## When this is not the problem

A frame with no `image_integrity` row in a project that has **never** been analysed is not this. Check `v_integrity_coverage` first: `coverage` near zero means the pass has not run, not that it failed.

## Related

* `AURA-ML-5017` - the same shape for the face pass.
* `AURA-ML-5027` - the same shape for scene classification.
* `AURA-ML-5032` - the same shape for grouping.
