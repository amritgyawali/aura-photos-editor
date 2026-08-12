# AURA-CLOUD-6004 - The AI provider rate-limited or is overloaded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. Progress does not stall.

## What actually happened

HTTP 429 or 503, usually with a `Retry-After` header. Either the account's requests-per-minute limit was hit, or the provider is shedding load.

## What AURA does automatically

Honours `Retry-After` when present, otherwise exponential backoff with jitter, up to the bounded attempt count. Requests that cannot wait fall back locally. Because the default policy is at most one call per 40 images, and contact sheets batch up to twelve decisions into one call, hitting a rate limit from AURA alone is unusual - suspect another tool sharing the key.

## Operator steps

1. Ask what else is using that key. A shared team key is the usual answer.
2. Check the provider's status page before assuming an account limit.
3. If it recurs on a dedicated key, lower the concurrency in Settings > AI Keys; the gateway will then make fewer, larger calls.

## Related

- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
