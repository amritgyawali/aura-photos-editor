# AURA-ML-5032 - One photograph could not be placed in a moment

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and the frame on its own in the grid. It is not hidden, not marked as a problem on the cell, and not excluded from anything downstream - it simply has no stack around it.

## What actually happened

Grouping needs two things about a photograph, and one of them was missing:

1. **An embedding.** Phase 05's perceptual pass has not reached this frame, or it was purged by a `MODEL_VER` bump and not yet recomputed. `SELECT photo_id FROM photo WHERE project_id = ? AND photo_id NOT IN (SELECT photo_id FROM embeddings)` is the list.
2. **A timeline time.** `photo.timeline_time IS NULL` - EXIF had no capture time and no file mtime was usable, or phase 01's clock alignment has not run for this card. Without it there is no cadence, no window and no candidate.

Both are *upstream* gaps rather than grouping failures, which is why the code is `ItemFailed`/`Retry` rather than degraded: the frame is retried by the next pass, and by then the missing thing may be there.

## What AURA does automatically

Leaves the frame **ungrouped rather than making it a moment of one**, and the distinction is the whole point of this code.

A moment of one that was compared against its neighbours and joined none of them is a real decision, and it is written with `SINGLETON_CONFIDENCE` and a reason that says "no neighbouring frame was close enough". A frame that was never compared with anything is not that, and writing it as a singleton would make the two indistinguishable - which would let a later phase count a phase 05 gap as a grouping decision.

So `v_moment_coverage` reports it instead: `groupable` counts frames that have an embedding, `grouped` counts frames in a moment, and the difference is this code's population. `MomentStatusDto` carries both, and the moments view says "N could not be placed and are shown on their own."

## Operator steps

1. Read the count from the moments view header rather than from the log - the log line is per pass and the header is the current truth.
2. `SELECT photos, groupable, grouped FROM v_moment_coverage WHERE project_id = ?`. `photos - groupable` is cause 1; `groupable - grouped` is cause 2 or a frame that legitimately joined nothing.
3. For cause 1, run the perceptual pass. For cause 2, check `SELECT COUNT(*) FROM photo WHERE project_id = ? AND timeline_time IS NULL`.
4. Re-run the grouping pass. It is resumable in the only sense that matters here: it recomputes everything unlocked from the catalog, so a frame that has since gained an embedding is simply included.

## When this is not the problem

A wedding where `groupable` equals `grouped` and some moments hold one frame has no instances of this code. Those are singletons by decision, and their reasons say so.

## Related

* `AURA-ML-5027` - the same shape for a frame that could not be classified into a scene.
* `AURA-ML-5030` - the whole-project version: a grouping whose *shape* is implausible rather than a frame that is missing from it.
