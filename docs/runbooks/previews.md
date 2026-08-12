# Preview troubleshooting

One page for "the thumbnails are wrong, slow or missing". Error-code runbooks
live beside this one (`AURA-RAW-2001` to `AURA-RAW-2008`, `AURA-IO-1009`); this
page is the map that says which one to open.

## How previews are produced

```text
photo row -> primary file -> tier 1 embedded JPEG   -> cache/<hh>/<hash>.p1.thumb.jpg
                          -> tier 2 2048 px proxy   -> cache/<hh>/<hash>.p1.proxy.jpg
                                                    -> cache/<hh>/<hash>.p1.linear.bin
                                                    -> cache/<hh>/<hash>.p1.meta.json
```

- The cache key is the file's BLAKE3 plus `pipeline_ver`. Renaming or moving a
  file keeps its previews; two copies of one frame share one entry.
- Nothing here is truth. Every entry can be rebuilt from the original file, so
  deleting the cache is always safe.
- The `preview` table records that an entry exists. A missing file is healed on
  the next request, not repaired by hand.

## Symptom: no thumbnails at all

1. Does the Problems list have rows? If so, open the runbook for the code shown.
2. Is the source drive connected? Previews read the original file; a disconnected
   card gives `AURA-IO-1001` or `AURA-IO-1003` from the locate step.
3. Check the cache directory exists and is writable - `AURA-IO-1009` appears in
   the log when it is not. The default location is `cache/` beside the catalog.
4. Check free disk space. A full disk is the most common cause of a cache that
   never grows.

## Symptom: thumbnails are slow to fill in

Expected: 4,000 embedded previews in two to three minutes on eight cores. If it
is much slower:

- **Which tier is being built?** Tier 1 is milliseconds per file; tier 2 is
  roughly a hundred times more expensive. A grid that is waiting on proxies is
  doing the expensive thing.
- **Is the source on a network drive or a USB 2 reader?** Tier 1 is IO-bound.
  Copy to local storage and compare.
- **Is the file missing its embedded preview?** `AURA-RAW-2003` means AURA had to
  render instead of copy, which is about twenty times slower.
- **Is the mosaic being decoded because the preview is unusable?** Check the
  sidecar's `tier1.render_path`: `embedded_jpeg` is the fast path,
  `demosaic_quarter` is not.

## Symptom: scrolling stutters while a batch runs

The pool leaves one core free and visible requests are served on the calling
thread, so this should not happen. If it does:

- Check `worker_count()` in the logs. On a two-core machine there is only one
  worker and one free core, and the margin is thin.
- Check the queue depth. A queue at capacity (8,192) means prefetch is being
  dropped, which is the backpressure working, not a fault.
- Confirm the grid is calling `cancel_previews` as cells scroll away; work for
  off-screen cells should be abandoned, not merely deprioritised.

## Symptom: colours look wrong

1. Open the Explain panel and check the badges. `profile=generic` means
   `AURA-RAW-2006` - no camera profile, colours may drift.
2. `source=embedded` on a proxy means `AURA-RAW-2007` - the render came from the
   camera's own JPEG, so it carries the camera's look rather than AURA's.
3. If neither badge is present, the render used a real matrix and the documented
   curve, and a colour complaint is a colour bug: capture the file and the
   sidecar and hand both to the Colour Scientist role.
4. Remember what tier 2 is *for*. It is deliberately flat and neutral - no
   contrast, no camera picture style. It is not meant to look like the camera's
   JPEG; it is meant to be the same across every brand.

## Symptom: the cache is enormous

- The default budget is 40 GB and eviction runs on every write, so it should not
  exceed that. Check `preview_stats` in the settings panel.
- Roughly 3.5 GB per thousand images is expected at default settings.
- A cache above its budget means eviction is failing - look for `AURA-IO-1009`
  in the log, usually a permissions problem on the cache directory.
- The Delete button is safe. Everything rebuilds.

## Symptom: previews are stale after an update

`pipeline_ver` is part of the cache key. When the rendering pipeline changes, the
version is bumped and every proxy is rebuilt on first use; the old entries stay
until they age out, which is why an update can briefly double the cache size.

If pixels look stale *without* a version bump, that is a cache-correctness bug,
not a stale cache: capture the entry and open a ticket.

## Useful commands

```bash
# Build previews for an existing catalog.
cargo run --release -p aura-cli -- previews --catalog CAT.sqlite --project NAME --level proxy

# The phase gate: fixtures, import, both tiers, cache proof, colour measurement.
cargo run --release -p aura-cli -- verify --phase 02 --work target/phase02-verify

# Generate synthetic RAWs to reproduce a decoder problem without client files.
cargo run --release -p aura-cli -- raw-fixtures --out /tmp/raw
```

## Related

- Camera support: `docs/camera-support.md`
- Colour pipeline: `docs/adr/ADR-0003-colour-pipeline.md`
- Decode backend: `docs/adr/ADR-0004-raw-decode-backend.md`
- Error registry: `crates/aura-core/errors.toml`
