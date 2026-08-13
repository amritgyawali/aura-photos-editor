# AURA-ML-5017 - One photograph could not be scanned for faces

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once, in the Problems panel. The photograph stays in the grid, keeps its previews, keeps every edit and exports normally. It simply has no people data.

## What actually happened

The face pass runs on the 2048 px proxy (`aura_vision::face::FACE_LEVEL`), and one of two things failed:

* **The proxy would not decode.** Either it was never built, or the RAW behind it is one of the containers this build does not decode (see `docs/camera-support.md`), or the cache entry is corrupt. The underlying code is an `AURA-RAW-2xxx` or `AURA-IO-1xxx` and is on the log line above this one.
* **The pipeline refused the buffer.** The detector requires 8-bit sRGB with dimensions that agree with the payload. A linear or tiled buffer is refused rather than converted, because converting here would be a second preprocessing path and `PREPROCESS_VER` exists to prevent exactly that.

## What AURA does automatically

Counts it in `ScanReport::failed`, logs it with the photograph's id, and continues. One unreadable frame out of four thousand does not end a pass.

**No `face_scan` row is written**, and that is deliberate rather than an oversight. The resumability ledger records that a photograph was *looked at*, so leaving it absent means the next pass tries again. That is right for a transient failure - a disconnected drive, a cache mid-purge - and harmless for a permanent one, which fails again cheaply.

## Operator steps

1. Read the code on the line above in the log. This one is a consequence; that one is the cause.
2. If it is a preview failure, rebuild the proxy: `just previews <catalog> <project> proxy`. `docs/runbooks/previews.md` covers the cache's self-healing.
3. If it is a decode failure, check `docs/camera-support.md`. Canon CRX, Panasonic RW2 and compressed RAF are undecoded in this build and fall back to the embedded JPEG; a file with no embedded JPEG has no tier 2 proxy at all.
4. If a whole card fails, suspect the card rather than the software, and copy it before formatting it.
5. To retry deliberately, re-run the face pass. The pending set is a query, so nothing has to be reset.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Preview troubleshooting: `docs/runbooks/previews.md`
- Camera coverage: `docs/camera-support.md`
- Model card: `docs/model-cards/face_detect.md`
