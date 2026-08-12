# AURA-RAW-2007 - Mosaic compression unsupported; the proxy came from the embedded preview

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. Affected images carry a `source=embedded` badge on the proxy in the Explain panel.

## What actually happened

The container opened and the sensor data was located, but its compression scheme is one this build cannot decode - see the per-format matrix in `docs/camera-support.md`. Rather than failing the image, AURA built the 2048 px proxy from the camera's embedded preview and marked it.

That distinction matters to the ML lane: models are trained on Tier 2 pixels rendered through the documented colour path, and an embedded-preview proxy carries the camera's baked-in look instead. Every score derived from such a proxy records the source, so a later re-run can be prioritised.

## What AURA does automatically

The proxy is produced, tagged `source = embedded` in the sidecar and the `preview` table, and `preview.decoded` is emitted with the real source. The image is not quarantined and the pipeline stays complete - this is the local-first fallback, working as designed.

## Operator steps

1. Check `docs/camera-support.md`. If the format is listed as *preview-only*, this is a known gap and the entry names the ticket that closes it.
2. Do not re-copy the files; the storage is fine.
3. When mosaic support for that format lands, bump `pipeline_ver` so affected proxies rebuild.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Support matrix: `docs/camera-support.md`
- Decode backend ADR: `docs/adr/ADR-0004-raw-decode-backend.md`
