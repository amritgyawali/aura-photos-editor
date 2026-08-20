# AURA-ML-5071 - Stored grades came from different heads, arithmetic or intents

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A message that AURA has improved how it grades and is re-checking the wedding in the
background. Everything keeps working while it does.

## What actually happened

Three version columns invalidate three different things, and this code is raised when a
comparison would cross any of them:

- **`model_ver`** invalidates the learned tone prediction.
- **`analysis_ver`** invalidates the histogram statistics, the curve fit, the content
  clustering, the HSL solve and the guards - everything this build computes.
- **`intent_ver`** invalidates the per-scene targets those numbers were compared against.

It is **degraded rather than blocking**. Stale decisions keep working, the outline reports
the lowest version present so a caller about to draw a conclusion over a mixed set finds out
before it draws it, and the background pass replaces the rows as it reaches them.

## Operator steps

1. Let the pass run. `ColourStore::pending` is keyed on all three versions, so the work
   remaining is a query and a killed pass resumes exactly where it stopped.
2. A photographer's own values are never re-derived. `user_edited` rows keep their numbers
   through the upsert.
3. If the message persists after a full pass, compare `ColourOutline`'s three versions with
   `Colour::current_versions`. A mismatch that survives a pass means rows are being written
   by something other than this build.
