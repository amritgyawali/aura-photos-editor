# AURA-CLOUD-6001 - No cloud AI key configured

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing interrupts them. Decisions in the Explain panel are marked `local_fallback`, and Settings > AI Keys shows "no key".

## What actually happened

A task asked the gateway for a cloud answer and no key exists in the OS credential store for the selected provider. This is the default state of a fresh installation and is not a fault.

## What AURA does automatically

The gateway short-circuits before building any payload - nothing is encoded, nothing is hashed, and no audit row is written for a call that never happened. The task's `local_fallback` answers, `source` is `local_fallback`, and a `cloud.fallback` telemetry event records the reason.

## Operator steps

1. Confirm the user actually wants cloud AI. The product is complete without it.
2. If they do: Settings > AI Keys, choose the provider, paste the key, press Check. A successful check writes the key to the OS store and shows the model tiers it resolved.
3. If the check fails, the code will be `AURA-CLOUD-6002` (key rejected) or `AURA-CLOUD-6003` (unreachable), not this one.

## Related

- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
- Guide: `docs/using-your-own-ai-key.md`
