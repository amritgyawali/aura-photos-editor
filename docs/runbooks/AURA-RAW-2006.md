# AURA-RAW-2006 - No colour profile for this camera; a generic matrix was used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The one-sentence message registered for this code in `errors.toml`. Affected images carry a `profile=generic` badge in the Explain panel.

## What actually happened

Tier 2 renders through a documented colour path: camera native -> XYZ (D50) -> Bradford adaptation -> linear Rec.2020. The camera-to-XYZ matrix comes from the file itself when it is a DNG, otherwise from the bundled profile table in `crates/aura-raw/src/colour/profile.rs`. Neither was available, so the generic matrix was used.

The generic matrix is a reasonable average, not a lie: it is documented, deterministic and identical for every file that falls back to it. But it is not this camera, so colour can drift.

## What AURA does automatically

`colour.profile_missing` is emitted with make and model, the sidecar records `"matrix": "generic"`, and every downstream decision that used those pixels can be re-run once a real profile ships. The image is not quarantined.

## Operator steps

1. Record the make and model exactly as EXIF spells them. That string is the lookup key.
2. Shoot or obtain a ColorChecker frame from that body and hand it to the Colour Scientist role; profiles are only added with that evidence and a sign-off (see `docs/adr/ADR-0003-colour-pipeline.md`).
3. After the profile ships, bump `pipeline_ver` so the cached proxies are rebuilt rather than silently kept.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Profile table: `crates/aura-raw/src/colour/profile.rs`
- Colour ADR: `docs/adr/ADR-0003-colour-pipeline.md`
- Camera support: `docs/camera-support.md`
