# AURA-RENDER-8025 - A file name could not be made unique

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA ran out of ways to give this photograph a name of its own. Change the file-naming template so it includes a number or the original name.

## What actually happened

The naming template produced a name that was already taken, and appending collision suffixes did not
find a free one within the writer's bound.

In practice this means a template with neither `{seq}` nor `{original}` in it over a set larger than
the suffix space. `DeliveryCode::NamingTemplateNotUnique` warns about the same template *before* the
job runs, which is where most people meet this.

## What to do

Add `{seq}` or `{original}` to the template. `preview_names` shows the whole set's names without
writing anything, which is the fastest way to see the problem.

## Where it comes from

PHASE-30. See `docs/adr/ADR-0061-delivery-learning-loop-and-release.md` and
`docs/plan/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md`.
