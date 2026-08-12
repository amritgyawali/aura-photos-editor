# AURA-CLOUD-6014 - The estimated cost of one call exceeded the task ceiling

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a local decision for that step.

## What actually happened

Every task declares `max_cost_usd`, defaulting to USD 0.02. The pre-call estimate, computed from the token count, the image count and the selected model's price row, exceeded it. This is a *per-call* ceiling and is separate from the project and monthly caps.

## What AURA does automatically

Tries the next cheaper tier the task still permits. If even the cheapest acceptable tier is over the ceiling, it refuses and falls back locally. **No call is made**, so nothing is billed.

## Operator steps

1. Look at what made the estimate large. A contact sheet of twelve 768 px tiles is roughly 12,000 image tokens; a task that passes full proxies instead of thumbnails is the classic cause.
2. Check the price table version in the audit row. A provider price rise with a stale table under-estimates; a stale table that is too expensive over-estimates and produces this code.
3. Raising a task's ceiling is a task version bump, because it changes the conditions the cache's existing entries were made under.

## Related

- Cost governor: `crates/aura-cloud/src/budget.rs`
