# AURA-ML-5120 - A scene has no cleanup policy row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Photographs of that kind carry no proposals. Everything else in the wedding is examined normally.

## What actually happened

`cleanup_policy.toml` loaded and has no row for this scene. Phase 07's vocabulary has 22 scenes and
the shipped file covers all of them, so this means either a locally edited file or a scene added by
a newer taxonomy than the policy file knows about.

## What AURA does about it

Leaves those photographs alone. It does **not** fall back on a neutral row, and that is deliberate:
invariant 7 says no threshold is global, and a default area cap applied to a scene nobody wrote a
row for is exactly a global threshold wearing a scene's name.

## The fix

Add the row, with a `reason` sentence. Bumping `policy_ver` re-examines the affected photographs.
