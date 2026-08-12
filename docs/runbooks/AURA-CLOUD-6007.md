# AURA-CLOUD-6007 - Cloud AI is switched off

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and an honest Settings panel listing what is reduced while cloud AI is off.

## What actually happened

Either the per-project switch is off, or global offline-studio mode is on. Offline-studio mode is the stronger of the two: it makes the whole crate inert, and no key is even read.

## What AURA does automatically

The gateway refuses before building a payload. Nothing is encoded and nothing is hashed. This is the same path the offline acceptance test exercises.

## Operator steps

1. Check the global switch first - a studio that turned it on for one sensitive client and forgot is the usual cause.
2. Then the per-project switch, which is deliberately off for new projects.
3. If both are on and this code still appears, the endpoint is region-pinned to a host the request did not match. That is also reported as this code, with `reason=region_pinned`.

## Related

- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
