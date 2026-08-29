# AURA-ML-5109 - Stored geometry plans came from different arithmetic or profile tables

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A background re-check of the wedding's framing, and nothing else. Framing they set themselves is
untouched.

## What actually happened

`geometry_plan` carries two version columns, `analysis_ver` and `profile_ver`, and a row was read
whose pair does not match the versions the running build decides at. **Two rather than three:**
this phase ships no model - the third since phase 08 - so the numbers that can move are the
arithmetic and the tables.

They invalidate different things, which is why they are separate columns:

* `analysis_ver` moves when the straightening band, the keystone solver, the crop objective, the
  improvement margin or the safety filter changes. Every field on the plan is then stale.
* `profile_ver` moves when `assets/lens_profiles/profiles.toml` or
  `crates/aura-geometry/config/crop_rules.toml` changes. The lens correction and which scenes may
  be cropped are then stale; the rotation is not.

Comparing a plan written at one pair against one written at another returns a plausible answer
that means nothing - a wedding half-planned under a stricter margin looks like a wedding where
AURA changed its mind about cropping.

## What to do

1. Nothing, usually. `GeometryStore::pending` returns every row that is not at the current pair,
   so the next pass re-plans them and the count shrinks to zero. A row with `user_edited = 1` is
   never re-planned and never becomes stale in a way that matters, because it is not AURA's
   decision.
2. If the count is not shrinking, a pass is not running. Start one - `geometry_pass` on the IPC
   surface, or `aura-cli verify --phase 23` for the fixture project.
3. If a *studio* changed `crop_rules.toml` and expected an immediate re-plan, remember that the
   loader refuses a file that loosens a bound the code owns (`AURA-ML-5112`); check for that
   first.
