# AURA-CLOUD-6010 - The provider returned an unrecognised response

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence and a `local_fallback` decision.

## What actually happened

The transport succeeded and the status was not one of the ones we map (401/403, 429, 5xx), but the envelope could not be parsed: an unexpected content type, an HTML error page from a proxy, a body that is not JSON, or a JSON body missing the fields the provider's own documented shape promises.

Distinct from `AURA-CLOUD-6005`: there the *content* was wrong, here the *envelope* was.

## What AURA does automatically

Falls back locally, and records the status code and the first 200 bytes of the body, redacted, in the audit row.

## Operator steps

1. An HTML body almost always means a corporate proxy or captive portal intercepted the request. Check the `via` and `server` headers in the audit detail.
2. A JSON body with a shape we do not expect means the provider changed their API. That is a code change in the relevant provider module plus a cassette re-record - escalate to MBE.
3. Check whether the same key works against the same endpoint with `curl`. That separates "our parser" from "their response".

## Related

- Provider modules: `crates/aura-cloud/src/anthropic.rs`, `openai.rs`, `google.rs`, `compat.rs`
