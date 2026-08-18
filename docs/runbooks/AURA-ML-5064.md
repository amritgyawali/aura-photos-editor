# AURA-ML-5064 - A scene has no exposure target row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Those photographs are exposed against the neutral band, carry the `no_target_row` reason in
the develop panel, and have a lower confidence than the rest.

## Why this is expected rather than exceptional

`scene_profiles.toml` grows a row whenever a tradition is added, and a scene can reach this
analyser before a product manager has written its exposure band - exactly as a camera body
ships before anybody has measured its MTF50 (`AURA-ML-5037`). The substitute here is a
*neutral* band rather than a cautious one, so the wedding is fully estimated and the
confidence drops by a fixed amount.

## Operator steps

1. Read the `scene` context field.
2. Add a row to `crates/aura-brain-photo/config/exposure_targets.toml` with a written
   rationale, and bump `version`.
3. The rows made under the old version become pending by definition and the background pass
   picks them up. Nobody has to trigger anything.
