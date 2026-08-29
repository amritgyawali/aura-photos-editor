# AURA-ML-5117 - A cleanup decision could not be recorded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Their accept or reject did not stick, with a message saying so. The photograph is unchanged either
way - a rejected proposal was never applied, and an accepted one that failed to record was not
applied either.

## What actually happened

`CleanupOverride` named no proposal, or asked for nothing. There are only three things a person can
say on this surface - accept, reject, and "turn cleanup off for this photograph" - because there is
no strength field, no size field and no description field anywhere in the contract.

That is why this error is so narrow: an override that is not one of those three is not a value out
of range, it is an empty request.

## What to check

* The `proposal_id` refers to a proposal that still exists. A re-analysis under a new
  `analysis_ver` replaces proposals nobody has decided on, so a panel holding a stale id will see
  this. Reloading the photograph is the fix.
* The panel sent at least one of `accept` or `disable_for_image`.
