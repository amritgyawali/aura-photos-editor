# AURA-ML-5110 - One photograph's geometry could not be worked out

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph delivered exactly as it was shot, and the rest of the wedding unaffected.

## What actually happened

The pass could not finish one frame. The commonest causes, in the order they occur:

* the proxy would not decode, so there were no pixels to measure a horizon or an edge profile in;
* the catalog has no width or height for the photograph, so a normalised rectangle cannot be
  turned into a resolution figure;
* the store refused the write - which is then a `AURA-DB-3006` underneath this one.

**A failure here is one row, never a pass.** `GeometryPass::run_ids` counts the frame in
`GeometryPassReport::failed` and moves on, because one unreadable frame must not end a wedding's
worth of work.

## What to do

1. The message names the photograph. Open it in the develop view: if the preview is also broken,
   this is a phase 02 decode problem rather than a geometry one, and
   `docs/runbooks/previews.md` is the right runbook.
2. Check the catalog has dimensions for the row:
   `SELECT width_px, height_px FROM photo WHERE photo_id = ?`. A NULL there is an ingest problem.
3. Re-run the pass. The frame is still pending, so it is retried without recomputing the wedding.
