# AURA-ML-5070 - The tone intent table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing is graded at all, and the Develop panel says AURA could not load the settings that
decide how much contrast each kind of photograph wants.

## What actually happened

`crates/aura-brain-photo/config/tone_intent.toml` failed validation. **Refusal is
whole-file**, exactly as phase 09's calibration loader and phase 15's target loader are, and
the consequence here is the reason: a table that loaded the ceremony rows and dropped the
reception rows would grade half a wedding against measured intents and half against neutral
ones, and the contrast would visibly change at a chapter boundary.

The loader names the file, the key and the rule it broke. The usual causes:

- a `rationale` shorter than nine characters - every row needs a written reason, because an
  intent nobody can explain is one nobody can argue with;
- a value outside its documented range, which the file's own header lists;
- a scene id that is not in phase 07's frozen taxonomy. Adding a scene is a contract change
  and needs an ADR;
- a scene appearing twice, which would make the answer depend on file order.

An installation *override* that fails leaves the shipped table in place and does not raise
this. Only a broken embedded baseline halts, and that is a build bug rather than a deployment
one.

## Operator steps

1. Read the error's detail. It names the exact key.
2. Restore the shipped file, or delete the installation override at
   `<config>/tone_intent.toml` to fall back to it.
3. Bump `version` in the file whenever a row changes. It is written into every decision, and
   a changed table with an unchanged version makes two incomparable grades look comparable.
