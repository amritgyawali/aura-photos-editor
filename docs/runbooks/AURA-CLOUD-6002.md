# AURA-CLOUD-6002 - The provider rejected the API key

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A toast with the registered sentence, and a red state on Settings > AI Keys. The wedding keeps processing.

## What actually happened

The provider answered 401 or 403. In order of likelihood: the key was revoked or rotated, the key belongs to a different provider than the one selected, the account has no credit, or the organisation header the key requires was not configured.

## What AURA does automatically

The gateway opens its circuit breaker for that provider for the rest of the session, so 3,000 images do not produce 75 identical rejections. Every task falls back locally. **The key is not deleted** - a revoked key and a typo look identical from here, and deleting the user's key on a 401 is not ours to do.

## Operator steps

1. Ask the user to re-paste the key. Trailing whitespace and a partially selected copy are the two most common causes.
2. Check the provider selector matches the key's issuer. An Anthropic key against the OpenAI provider presents exactly this way.
3. Have them confirm billing is active on the provider's own dashboard.
4. Press Check. A pass closes the breaker immediately; there is no need to restart.

## Related

- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
- Key storage: `crates/aura-cloud/src/keys.rs`
