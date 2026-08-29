# AURA-ML-5095 - No lens profile matched, so the optics were left alone

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs from one lens with no distortion correction and no fringing correction, carrying
the `lens_profile_missing` reason. Everything else about them is normal. Nothing is damaged
and nothing is guessed.

## What actually happened

Section 6.1 gives three routes to a lens correction, in order of preference:

1. **Embedded.** The camera wrote correction data into the file. Preferred, because the
   manufacturer measured the lens.
2. **The bundled table.** `assets/lens_profiles/` keyed by lens id and focal length.
3. **Estimation.** No profile, so the distortion is fitted from long straight edges in the
   frame itself.

This code is raised when all three fail: no embedded data, no table entry, and fewer than
`lens::MIN_EDGES` straight edges to fit from. A reception frame of a dance floor has no
straight edges worth the name, which is the common case.

**When route 3 does succeed, chromatic aberration is still withheld** -
`GeometryCode::CaWithheld`. A CA correction fitted from the same edges it is meant to clean
will happily invent fringing of the opposite colour, and a photographer looking at a purple
rim they did not have before has been actively harmed rather than merely unhelped.
`LensSource::is_measured` is the predicate, and it is why the enum distinguishes `Profile`
from `Estimated` at all.

## What to do

Nothing urgent. The photographs are usable. If the lens is a common one, the missing id is
worth adding to the bundled table - section 12's mitigation is "telemetry on missing lenses
and a monthly profile expansion task", and `GeometryOutline::missing_profiles` is that
telemetry.

## How to confirm

`GeometryOutline::missing_profiles` in the Geometry panel header lists the twenty most
frequent, or:

```sql
SELECT lens_id, COUNT(*) FROM geometry_plan
WHERE  lens_source = 'none' AND lens_id IS NOT NULL
GROUP  BY 1 ORDER BY 2 DESC;
```

## Adding a profile

`assets/lens_profiles/README.md` documents the format and the attribution requirement. A
profile is a measurement somebody made; the file records who.
