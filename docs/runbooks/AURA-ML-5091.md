# AURA-ML-5091 - A retouch override or a protected-feature change was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The change did not take. Nothing else moved: this code is raised *before* anything is written.

## What actually happened

One of five things:

* the photograph has no retouch plan yet, so there is nothing to override;
* the override sets nothing at all;
* a strength is outside `0..1`;
* the identity is not one this project knows;
* **the protected feature is absolute.**

The last is the one worth reading twice. `ProtectedKind::is_absolute` is true for
`ProtectedKind::Tattoo`, and `RetouchService::set_protection` refuses to clear one. Section 10.1
of PHASE-20 gates tattoo removal at **zero** per cent, and section 11 of `docs/plan/CLAUDE.md`
permanently forbids operations that change a person's identity. A promise a setting can retract
is not a promise, so this is a property of the kind rather than a default.

## What to do

1. For a strength or preset refusal, check the value is in range and that the frame has been
   through the retouch pass.
2. For a tattoo, there is nothing to do and nothing has gone wrong. AURA does not alter tattoos
   and will not stop protecting one because it was asked to.
3. For any other permanent feature - a mole, a freckle field, a scar, a birthmark, a dimple -
   clearing the protection is allowed and takes effect across the whole gallery, because a
   protect row belongs to a person rather than to a photograph.
