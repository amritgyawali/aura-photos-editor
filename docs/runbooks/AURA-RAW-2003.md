# AURA-RAW-2003 - No embedded preview in the file; one was rendered instead

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. Thumbnails still appear; they take longer for the affected files.

## What actually happened

Tier 1 of the preview pyramid copies the JPEG the camera already stored inside the RAW, which costs a few milliseconds. Some medium-format backs and some DNG converters store no preview at all. For those files AURA falls back to a quarter-size demosaic, which is correct but roughly twenty times slower.

## What AURA does automatically

The fallback runs, the resulting preview is tagged `source = decoded` in the `preview` table and in the cache sidecar, and QA can therefore tell the two paths apart. Nothing is quarantined; this is a warning, not a failure.

## Operator steps

1. No action is required. If a whole shoot is affected, expect the first pass over the grid to be slower and say so to the photographer.
2. If the camera *should* embed previews, check whether the files went through a converter that strips them, and change that step.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Tier 1: `crates/aura-raw/src/thumb.rs`
- Preview troubleshooting: `docs/runbooks/previews.md`
