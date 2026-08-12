# AURA-CLOUD-6006 - The project or monthly spending cap was reached

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The spend meter at 100 %, the registered sentence, and the run continuing to completion.

## What actually happened

The cost governor's pre-call estimate would have taken spend past the cap. This is the designed behaviour of a hard stop, not a failure.

## What AURA does automatically

Stops calling. Every remaining decision uses its local fallback and is recorded as downgraded, so the Explain panel can show exactly which decisions would have been better with more budget. Spend state is durable, so killing and resuming the run does not reset it.

## Operator steps

1. Show the user Settings > AI Keys. The per-project and per-month caps are separate; check which one was hit.
2. Raising the cap and re-running is cheap: the response cache means every decision already made is free the second time, and only the downgraded ones are asked again.
3. If a 3,000-image wedding hit a USD 1.50 cap, something is calling more often than policy allows. Check the audit table for a task making more than one call per 40 images.

## Related

- Cost governor: `crates/aura-cloud/src/budget.rs`
- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
