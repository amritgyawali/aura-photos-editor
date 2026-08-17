# AURA-ML-5055 - The autonomy band table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA has stopped. It does not analyse, cull or act, and it says that it could not load the
settings that decide how much it is allowed to do on its own.

## Why this halts

Because the alternative is worse. The band table maps a confidence to what the product may
do without asking. A build that fell back to a default when the file would not load would be
a build granting itself autonomy from a bug, and section 6.4's whole argument is that the
bands are a safety mechanism rather than a preference.

## What was refused, and why

`crates/aura-explain/config/autonomy_bands.toml`, for one of four reasons the `detail` names:

1. the file is not valid TOML;
2. a `[[kind]]` row names something that is not one of the six decision kinds;
3. a row's thresholds do not descend - `auto_at` below `zero_touch_at`, or `zero_touch_at`
   below `suggest_at`;
4. a row has no written reason. Every row in every configuration table in this product
   carries one, because a threshold nobody can explain is a threshold nobody can defend to a
   photographer whose gallery it changed.

## Operator steps

1. If an installation override exists, remove it. The shipped table is embedded in the
   binary and returns as soon as the override is gone.
2. If the shipped table itself is at fault, the build is broken; reinstall.
3. After any edit, bump `version` in the same commit. It is recorded on every decision the
   table banded, and a support case that cannot tell which thresholds were in force is a
   support case about the wrong build.
