# AURA-ML-5134 - A bundled brand baseline could not be loaded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing was changed about the cameras of that manufacturer, and the report says AURA has no
measurements for them.

## What actually happened

One file in `assets/camera_baselines/` would not parse, named an unknown brand slug, or declared a
departure outside **half** of a contract ceiling.

That half is not a typo. A baseline states one brand's departure from a neutral reference, and the
correction between two brands is the composition of two of them - so two brands each at a full
ceiling in opposite directions would compose to twice it and be clamped, which would silently turn a
measurement into a bound. Half is the largest declaration that can never do that.

## Why this is a warning while a bad policy table halts

A policy file governs **every** body in the wedding. A baseline governs **one brand**, and the
fallback when it will not load is the neutral baseline, which is the identity: AURA changes nothing
about that manufacturer's bodies rather than guessing. One degrades a camera to no correction, the
other would silently re-scope the phase.

## What these files are in this build

**Fabricated.** `measured = false` in every one of them. There is no photographed colour target in
this repository, no laboratory and no camera; the numbers were chosen to be plausible from published
behaviour of each manufacturer's rendering. `baseline::tests::nothing_in_this_build_was_measured`
asserts it, and `docs/camera-matching.md` says so in the product's own voice.

A body matched from one carries `CameraCode::BaselineOnly` and the report leads with it.

## Fixing it

The detail line names the file, the table and the offending key. Restore the file from the
repository. A studio replacing baselines with their own measurements should replace them **per
brand**: unlike the policy table, this directory is eight independent measurements and a studio that
has measured two bodies should not have to fabricate six more.
