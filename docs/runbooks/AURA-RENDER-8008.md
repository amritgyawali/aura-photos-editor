# AURA-RENDER-8008 - The camera profile is unknown

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs from one camera body render with slightly flat or slightly cool colour, and a
note says AURA has no colour profile for that camera and used a neutral one.

## What it means

`aura_render::profiles` has no entry for the EXIF camera model, so the reference profile - a
neutral D65 matrix - renders it. The image is correct in the sense that nothing is clipped
or shifted arbitrarily; it is uncalibrated in the sense that the camera's own spectral
response has not been accounted for.

## Operator steps

1. The context carries `camera`, exactly as EXIF spells it.
2. `crates/aura-render/config/camera_profiles.toml` is the table. Adding a body is a row
   with its `xyz_to_camera` matrix and the illuminant it was measured under.
3. A DNG from the same body carries `ColorMatrix1` and `ColorMatrix2` in its tags; those are
   the numbers to add.
4. Adding a row changes rendered colour for every photograph from that body, so it bumps
   `profiles::PROFILES_VER` and re-renders. Do not add a row without measuring it - a wrong
   matrix is worse than the neutral fallback, because it looks deliberate.
